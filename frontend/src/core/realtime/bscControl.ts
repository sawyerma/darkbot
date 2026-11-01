import { create as message, fromBinary, toBinary, type MessageInitShape } from "@bufbuild/protobuf";
import { create } from "zustand";
import { CommandRequestSchema, CommandResponseSchema, CommandLookupResponseSchema, CommandStatus, CommandPhase } from "../generated/darkbot/control/v1/commands_pb";
import { ProjectId } from "../generated/darkbot/control/v1/topics_pb";
import { BscLiveGrantCommandSchema } from "../generated/darkbot/bsc/v1/status_pb";

// The one BSC command client (Audit E 4.12): the readiness job, the offline G0.7 candidate
// request and the requests of the native bsc_runtime owner share one transport — the session's
// CSRF token, one POST per user action, and after a lost acknowledgement exactly one lookup,
// never a second POST. Each control keeps its own state of what the push must show next.

const positiveMs = (raw: unknown, fallback: number) => {
  const n = Number(raw);
  return Number.isSafeInteger(n) && n > 0 ? n : fallback;
};
/** How long the browser waits for the gateway's answer to one command (Audit E 3.8): longer than
 *  the gateway's own controller bound (DARKBOT_GATEWAY_CONTROLLER_TIMEOUT_MS, 50 s by default), so
 *  that the gateway answers first. One value, set at build time with VITE_COMMAND_TIMEOUT_MS. */
export const COMMAND_TIMEOUT_MS = positiveMs(import.meta.env.VITE_COMMAND_TIMEOUT_MS, 60_000);
/** A session bootstrap or a receipt lookup is a local gateway read: a tenth of the command bound. */
export const READ_TIMEOUT_MS = Math.ceil(COMMAND_TIMEOUT_MS / 10);

const options: RequestInit = { credentials: "same-origin", cache: "no-store" };
const errorText = (error: unknown) => error instanceof Error ? error.message : String(error);

async function session(): Promise<string> {
  const r = await fetch("/api/v1/session", { ...options, signal: AbortSignal.timeout(READ_TIMEOUT_MS) });
  const token = r.headers.get("x-csrf-token");
  if (!r.ok || !token) throw new Error("Session unavailable");
  return token;
}
/** The ledger's phase of a recorded command (UNKNOWN when the gateway recorded none). */
async function lookup(id: string): Promise<{ phase: CommandPhase; detail: string }> {
  const r = await fetch(`/api/v1/command?project=${ProjectId.BSC}&request_id=${encodeURIComponent(id)}`, { ...options, signal: AbortSignal.timeout(READ_TIMEOUT_MS) });
  if (!r.ok) throw new Error(`Command lookup HTTP ${r.status}`);
  const c = fromBinary(CommandLookupResponseSchema, new Uint8Array(await r.arrayBuffer()));
  return { phase: c.found ? c.phase : CommandPhase.UNKNOWN, detail: c.detail };
}
/** What became of one command: refused by the gateway (nothing dispatched), decided (the POST's
 *  phase, or the one lookup's), UNKNOWN (it may have arrived; stays locked) or FAILED (not sent). */
type Outcome =
  | { kind: "REJECTED"; detail: string }
  | { kind: "DECIDED"; phase: CommandPhase; detail: string }
  | { kind: "UNKNOWN"; detail: string }
  | { kind: "FAILED"; detail: string };
interface BscRequest {
  requestId: string; action: string; resourceId?: string; params?: Uint8Array;
  expected?: MessageInitShape<typeof CommandRequestSchema>["expected"];
}
/** One BSC command, posted once. `ledgerDetail`: an accepted command's detail is its ledger
 *  receipt line (one lookup; the POST's phase when that fails). */
async function send(request: BscRequest, ledgerDetail = false): Promise<Outcome> {
  let submitted = false;
  try {
    const csrf = await session();
    const body = toBinary(CommandRequestSchema, message(CommandRequestSchema, { ...request, project: ProjectId.BSC, issuedAtUnixNs: BigInt(Date.now()) * 1_000_000n }));
    submitted = true;
    const r = await fetch("/api/v1/command", { ...options, method: "POST", headers: { "Content-Type": "application/x-protobuf", "X-CSRF-Token": csrf }, body, signal: AbortSignal.timeout(COMMAND_TIMEOUT_MS) });
    if (!r.ok) throw new Error(`Command HTTP ${r.status}`);
    const c = fromBinary(CommandResponseSchema, new Uint8Array(await r.arrayBuffer()));
    if (c.status !== CommandStatus.ACCEPTED) return { kind: "REJECTED", detail: c.detail };
    const posted: Outcome = { kind: "DECIDED", phase: c.phase, detail: c.detail };
    return ledgerDetail ? await lookup(request.requestId).then(l => ({ kind: "DECIDED" as const, ...l }), () => posted) : posted;
  } catch (error) {
    if (!submitted) return { kind: "FAILED", detail: errorText(error) };
    // Lost acknowledgement: one targeted lookup, never a second POST.
    try { return { kind: "DECIDED", ...await lookup(request.requestId) }; } catch { return { kind: "UNKNOWN", detail: errorText(error) }; }
  }
}

// ---------------------------------------------------------------- the readiness job
export type ProcessState = "RUNNING" | "STOPPED" | "STOPPING" | "FAILED" | "UNKNOWN";
export interface ReadinessProcess { state: ProcessState; run_id: string; pid: number | null; reason: string; observed_unix_ms: number; controller_available: boolean }
export type ReadinessAction = "bsc.readiness.start" | "bsc.readiness.stop";
/** A supervisor job as pushed in bsc.status or overview.aggregates. */
export interface PushedJob { state: string; launchId: string; resultCode: string; controllerAvailable?: boolean }
export type JobSource = "bsc" | "overview";
interface Receipt { id: string; phase: string; detail: string }
/** The effect a decided command must show in the pushed job before the control unlocks. */
interface Awaiting { action: ReadinessAction; runId: string }
interface ReadinessState { process: ReadinessProcess | null; busy: boolean; receipt: Receipt | null; sources: Record<JobSource, ReadinessProcess | null>; awaiting: Awaiting | null }
export const useReadinessControl = create<ReadinessState>(() => ({ process: null, busy: false, receipt: null, sources: { bsc: null, overview: null }, awaiting: null }));
const receiptKey = "darkbot.bsc.lastCommand";
const STATES: ProcessState[] = ["RUNNING", "STOPPED", "STOPPING", "FAILED", "UNKNOWN"];
function forget(): void { try { sessionStorage.removeItem(receiptKey); } catch { /* storage may be unavailable */ } }
/** Whether the pushed job shows the effect of the awaited command. */
function settled(a: Awaiting, p: ReadinessProcess | null): boolean {
  if (!p) return false;
  // A start is only offered from STOPPED/FAILED, so a running job or a new launch is its effect.
  return a.action === "bsc.readiness.start"
    ? p.run_id !== a.runId || p.state === "RUNNING" || p.state === "STOPPING"
    : p.run_id !== a.runId || p.state === "STOPPED" || p.state === "FAILED";
}
/**
 * Process truth comes only from the pushed supervisor job of a live topic (bsc.status on /bsc,
 * overview.aggregates on /); a stale, detached or missing source makes it UNKNOWN. No HTTP polling.
 */
export function applyReadinessJob(source: JobSource, job: PushedJob | null): void {
  const value: ReadinessProcess | null = job ? {
    state: STATES.includes(job.state as ProcessState) ? job.state as ProcessState : "UNKNOWN",
    run_id: job.launchId, pid: null, reason: `launchd readiness job${job.resultCode ? ` · result ${job.resultCode}` : ""}`,
    observed_unix_ms: Date.now(), controller_available: job.controllerAvailable === true,
  } : null;
  useReadinessControl.setState(s => {
    const sources = { ...s.sources, [source]: value };
    const process = sources.bsc ?? sources.overview;
    if (s.awaiting && settled(s.awaiting, process)) { forget(); return { sources, process, awaiting: null, busy: false }; }
    return { sources, process };
  });
}
/** Applies a decided ledger phase; open or undecided commands keep the control locked. */
function decideReadiness(id: string, phase: CommandPhase, detail: string, awaiting: Awaiting): void {
  const name = CommandPhase[phase] ?? "UNKNOWN";
  if (phase === CommandPhase.FAILED) { forget(); useReadinessControl.setState({ busy: false, awaiting: null, receipt: { id, phase: name, detail } }); return; }
  if (phase === CommandPhase.COMPLETED || phase === CommandPhase.PENDING || phase === CommandPhase.PENDING_RECONCILE) {
    useReadinessControl.setState(s => settled(awaiting, s.process) && phase !== CommandPhase.PENDING_RECONCILE
      ? (forget(), { busy: false, awaiting: null, receipt: { id, phase: name, detail } })
      : { busy: true, awaiting, receipt: { id, phase: name, detail } });
    return;
  }
  // Not recorded by the gateway: nothing was dispatched, the control is free again.
  forget(); useReadinessControl.setState({ busy: false, awaiting: null, receipt: { id, phase: name, detail } });
}
let recovered = false;
/** After a reload, recovers an unresolved command once; resolved commands are never looked up. */
export function recoverReadinessCommand(): void {
  if (recovered) return;
  recovered = true;
  let stored: Awaiting & { id: string } | null = null;
  try { const raw = sessionStorage.getItem(receiptKey); stored = raw ? JSON.parse(raw) as Awaiting & { id: string } : null; } catch { stored = null; }
  if (!stored?.id || useReadinessControl.getState().receipt) return;
  const { id, action, runId } = stored;
  useReadinessControl.setState({ busy: true, awaiting: { action, runId }, receipt: { id, phase: "RECOVERING", detail: action } });
  void session().then(() => lookup(id)).then(l => decideReadiness(id, l.phase, l.detail, { action, runId }))
    .catch(error => useReadinessControl.setState({ busy: true, receipt: { id, phase: "UNKNOWN", detail: errorText(error) } }));
}
export async function readinessCommand(action: ReadinessAction): Promise<void> {
  if (useReadinessControl.getState().busy) return;
  const id = crypto.randomUUID();
  const runId = useReadinessControl.getState().process?.run_id ?? "";
  const awaiting: Awaiting = { action, runId };
  try { sessionStorage.setItem(receiptKey, JSON.stringify({ id, ...awaiting })); } catch { /* commands also work without browser storage */ }
  useReadinessControl.setState({ busy: true, awaiting: null, receipt: { id, phase: "SENDING", detail: action } });
  const o = await send({ resourceId: runId, requestId: id, action });
  switch (o.kind) {
    case "REJECTED": forget(); useReadinessControl.setState({ busy: false, receipt: { id, phase: "REJECTED", detail: o.detail } }); return;
    case "DECIDED": decideReadiness(id, o.phase, o.detail, awaiting); return;
    case "UNKNOWN": useReadinessControl.setState({ busy: true, awaiting, receipt: { id, phase: "UNKNOWN", detail: o.detail } }); return;
    case "FAILED": forget(); useReadinessControl.setState({ busy: false, receipt: { id, phase: "FAILED", detail: o.detail } });
  }
}

// ---------------------------------------------------------------- the G0.7 A/B candidate request
// Offline only. The gateway records it; the native store applies or rejects it at the next
// desktop sync. ACCEPTED is never an effective switch, approval, signature or send.
export type CandidateProfile = "SELF_A" | "SELF_B";
/** busy: no decided ledger phase yet. awaiting: recorded, the native mirror must still show it. */
interface CandidateState { busy: boolean; receipt: Receipt | null; awaiting: string | null }
export const useCandidateControl = create<CandidateState>(() => ({ busy: false, receipt: null, awaiting: null }));
/** PENDING and PENDING_RECONCILE stay open and locked until the native decision is known. */
function decideCandidate(id: string, phase: CommandPhase, detail: string): void {
  const name = CommandPhase[phase] ?? "UNKNOWN";
  const open = phase === CommandPhase.PENDING || phase === CommandPhase.PENDING_RECONCILE;
  useCandidateControl.setState({ busy: open, awaiting: phase === CommandPhase.COMPLETED ? id : null, receipt: { id, phase: name, detail } });
}
/** The pushed native mirror shows this request: from now on its own state locks the selector. */
export function candidateMirrorShows(requestId: string | undefined): void {
  const { awaiting } = useCandidateControl.getState();
  if (awaiting && requestId === awaiting) useCandidateControl.setState({ awaiting: null });
}
/** One request per user action; the generation is the CAS of the displayed native state. */
export async function candidateCommand(profile: CandidateProfile, generation: bigint): Promise<void> {
  const state = useCandidateControl.getState();
  if (state.busy || state.awaiting) return;
  const id = crypto.randomUUID();
  useCandidateControl.setState({ busy: true, awaiting: null, receipt: { id, phase: "SENDING", detail: profile } });
  const o = await send({ requestId: id, action: "bsc.candidate.select", resourceId: profile, expected: { version: { case: "profileVersion", value: generation } } });
  switch (o.kind) {
    case "DECIDED": decideCandidate(id, o.phase, o.detail); return;
    case "UNKNOWN": useCandidateControl.setState({ busy: true, receipt: { id, phase: "UNKNOWN", detail: o.detail } }); return;
    default: useCandidateControl.setState({ busy: false, receipt: { id, phase: o.kind, detail: o.detail } });
  }
}

// ---------------------------------------------------------------- the native owner's requests
// Plan §5a.2, v9 §2, order V2 §5, plan 27.10.: source, component and venue modes and the wallet
// and economic switches of the one native bsc_runtime owner, which applies or refuses each with
// the shown generation as CAS; its one-line receipt is the detail. A requested LIVE is effective
// only while the owner's LIVE gate is clear (plan 28.10.: the grant of a confirmed scope, every
// switch, the limits and the verified qualification); an ENABLED switch or a grant is a
// permission only: every LIVE intent is admitted by the owner again at its dispatch.
export type SourceMode = "OFF" | "SHADOW" | "LIVE";
export type SwitchMode = "DISABLED" | "ENABLED";
/** The allowlisted gateway actions of the owner and the CAS each states. */
export type OwnerAction = "bsc.source.mode.set" | "bsc.venue.mode.set" | "bsc.wallet.set" | "bsc.economic.set" | "bsc.runtime.mode.set" | "bsc.attempt.limit.set" | "bsc.attempt.total.set" | "bsc.live.revoke";
/** `origin` names the control group that sent it; each shows only its own receipt. */
interface OwnerReceipt extends Receipt { origin: string }
interface SourceControlState { busy: boolean; receipt: OwnerReceipt | null }
export const useSourceControl = create<SourceControlState>(() => ({ busy: false, receipt: null }));
/** One request per user action; the generation is the CAS of the displayed native state
 *  (`walletPolicyVersion` for a wallet switch, `configVersion` otherwise). */
export async function ownerCommand(origin: string, action: OwnerAction, id: string, mode: string, generation: bigint): Promise<void> {
  const version = action === "bsc.wallet.set"
    ? { case: "walletPolicyVersion" as const, value: generation }
    : { case: "configVersion" as const, value: generation };
  await ownerSend(origin, action, `${id}:${mode}`, new Uint8Array(), version, `${id} ${mode}`);
}
/** The scope of a LIVE grant as the operator confirms it (plan 28.10.). */
export interface LiveGrantScope {
  venue: string; strategies: string; sources: string[]; adapters: string[];
  maxInputWei: string; maxSpendWei: string; maxAttempts: number; expiresUnixMs: bigint;
  /** BSC3-058: the exits of open positions alone, no BUY (a separate authorization). */
  exitOnly: boolean;
  /** BSC3-069: the execution risk the operator grants, every bound. */
  risk: LiveGrantRiskScope;
  /** 31.10. order: the positions and losses the operator grants, every bound. */
  limits: LiveGrantLimitsScope;
}
/** The positions and losses of a LIVE grant: a BUY beyond any of them is refused, exits go on. */
export interface LiveGrantLimitsScope {
  maxOpenPositions: number; maxRealizedLossWei: string; maxDailyRealizedLossWei: string; minBuyIntervalMs: bigint;
}
/** The execution risk of a LIVE grant (BSC3-069): the executor arms only within it. */
export interface LiveGrantRiskScope {
  maxBuySlippageBps: number; maxExitSlippageBps: number; maxGasPriceWei: bigint;
  maxBuyGasLimit: bigint; maxApproveGasLimit: bigint; maxSellGasLimit: bigint;
}
/** `bsc.live.grant`: the confirmed scope with the SHA-256 of its canonical text the operator was
 *  shown, at the owner generation; the native owner recomputes the digest. */
export async function liveGrantCommand(origin: string, scope: LiveGrantScope, scopeSha256: string, generation: bigint): Promise<void> {
  const params = toBinary(BscLiveGrantCommandSchema, message(BscLiveGrantCommandSchema, { ...scope, scopeSha256 }));
  await ownerSend(origin, "bsc.live.grant", "BSC_LIVE_GRANT", params, { case: "configVersion" as const, value: generation }, `BSC_LIVE_GRANT ${scopeSha256.slice(0, 12)}`);
}
async function ownerSend(origin: string, action: OwnerAction | "bsc.live.grant", resourceId: string, params: Uint8Array,
  version: { case: "walletPolicyVersion" | "configVersion"; value: bigint }, detail: string): Promise<void> {
  if (useSourceControl.getState().busy) return;
  const requestId = crypto.randomUUID();
  useSourceControl.setState({ busy: true, receipt: { id: requestId, phase: "SENDING", detail, origin } });
  // The decided phase arrives with the response; its receipt line is the ledger detail.
  const o = await send({ requestId, action, resourceId, params, expected: { version } }, true);
  const busy = o.kind === "UNKNOWN" || (o.kind === "DECIDED" && (o.phase === CommandPhase.PENDING || o.phase === CommandPhase.PENDING_RECONCILE));
  const phase = o.kind === "DECIDED" ? CommandPhase[o.phase] ?? "UNKNOWN" : o.kind;
  useSourceControl.setState({ busy, receipt: { id: requestId, phase, detail: o.detail, origin } });
}
/** A source or component mode request (the existing action). */
export const sourceCommand = (source: string, mode: SourceMode, generation: bigint, origin = "bsc-source") =>
  ownerCommand(origin, "bsc.source.mode.set", source, mode, generation);
