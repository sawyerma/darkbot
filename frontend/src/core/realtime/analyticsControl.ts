import { create as message, fromBinary, toBinary } from "@bufbuild/protobuf";
import { create } from "zustand";
import { CommandLookupResponseSchema, CommandPhase, CommandRequestSchema, CommandResponseSchema, CommandStatus } from "../generated/darkbot/control/v1/commands_pb";
import { ProjectId } from "../generated/darkbot/control/v1/topics_pb";
import {
  BacktestJobCommandSchema, BootNetworkCommandSchema, BootOfflineCommandSchema, BootStartCommandSchema, OptimizeJobCommandSchema,
  PostCaptureJobCommandSchema,
} from "../generated/darkbot/project0/v1/analytics_pb";
import { CapabilityControlCommandSchema, SourceControlCommandSchema } from "../generated/darkbot/project0/v1/owner_pb";

/**
 * B03/U-9: the analytics job commands (POST /api/v1/command → gateway → `darkbotctl p0 job` →
 * the same `p0ctl job` a terminal runs). ACCEPTED/COMPLETED only means the controller recorded
 * the job and started its worker; the job's progress and end are its state in
 * project0.analytics.jobs. A lost acknowledgement is looked up once, never posted again.
 */
export type JobAction = "project0.analytics.backtest.start" | "project0.analytics.optimize.start" | "project0.analytics.job.cancel"
  | "project0.capture.verify.start" | "project0.source.set" | "project0.capability.set"
  | "project0.boot.offline" | "project0.boot.network" | "project0.boot.start" | "project0.boot.cancel";
interface Receipt { id: string; action: JobAction; phase: string; detail: string }
interface AnalyticsControlState { busy: boolean; receipt: Receipt | null }
export const useAnalyticsControl = create<AnalyticsControlState>(() => ({ busy: false, receipt: null }));
const options: RequestInit = { credentials: "same-origin", cache: "no-store" };

/** The typed inputs of one backtest point, as the operator entered them. */
export interface PointInput {
  runId: string;
  minProfitUsd: string;
  /** Whole USD, comma separated. */
  capitalUsd: string;
  /** `all` or comma separated funding classes. */
  funding: string;
  maxSeconds: string;
  maxRamMib: string;
}
export interface GridInput { minProfitUsdGrid: string; fundingSets: string; holdoutBps: string; maxPoints: string; saveProfile: boolean }

const USD = /^\d{1,15}(?:\.\d{1,6})?$/;
const CLASS = /^[A-Z0-9_]{1,32}$/;
const positive = (v: string): bigint | null => (/^[1-9]\d{0,18}$/.test(v.trim()) ? BigInt(v.trim()) : null);
const list = (v: string, sep = ",") => v.split(sep).map((x) => x.trim()).filter((x) => x !== "");
const classes = (v: string): string[] | null => {
  if (v.trim() === "all") return [];
  const c = list(v);
  return c.length > 0 && c.length <= 8 && c.every((x) => CLASS.test(x)) ? c : null;
};

/** The typed point, or why it is not one (the controller validates it again). */
export function backtestCommand(p: PointInput): { params: Uint8Array } | { problem: string } {
  const capital = list(p.capitalUsd).map(positive);
  const funding = classes(p.funding);
  const seconds = positive(p.maxSeconds);
  const mib = positive(p.maxRamMib);
  if (!p.runId) return { problem: "Select a completed native run." };
  if (!USD.test(p.minProfitUsd.trim())) return { problem: "Minimum profit: a USD amount with at most 6 decimals." };
  if (capital.length === 0 || capital.length > 32 || capital.some((c) => c === null)) return { problem: "Capital: 1–32 positive whole USD amounts, comma separated." };
  if (funding === null) return { problem: "Funding: all, or funding class names." };
  if (seconds === null || mib === null) return { problem: "Budget: positive seconds and MiB are required (no hidden default)." };
  return {
    params: toBinary(BacktestJobCommandSchema, message(BacktestJobCommandSchema, {
      runId: p.runId, minProfitUsd: p.minProfitUsd.trim(), capitalUsd: capital as bigint[], funding,
      maxSeconds: seconds, maxRamBytes: mib * 1_048_576n,
    })),
  };
}

export function optimizeCommand(p: PointInput, g: GridInput): { params: Uint8Array } | { problem: string } {
  const point = backtestCommand(p);
  if ("problem" in point) return point;
  const grid = list(g.minProfitUsdGrid);
  const sets = list(g.fundingSets, ";").map(classes);
  const holdout = positive(g.holdoutBps);
  const points = positive(g.maxPoints);
  if (grid.length === 0 || grid.length > 64 || !grid.every((v) => USD.test(v))) return { problem: "Grid: 1–64 USD amounts, comma separated." };
  if (sets.length === 0 || sets.length > 16 || sets.some((s) => s === null)) return { problem: "Funding sets: separated by ';', each all or class names." };
  if (holdout === null || points === null) return { problem: "Holdout bps and maximum points are required." };
  const baseline = fromBinary(BacktestJobCommandSchema, point.params);
  return {
    params: toBinary(OptimizeJobCommandSchema, message(OptimizeJobCommandSchema, {
      baseline, minProfitUsdGrid: grid, fundingSets: (sets as string[][]).map((classes) => ({ classes })),
      holdoutBps: holdout, maxPoints: points, saveProfile: g.saveProfile,
    })),
  };
}

/** A08/F-02: verification off, or strict from the cache (online strict is a terminal action). */
export function verifyCommand(v: { runId: string; mode: "off" | "strict-cache"; resume: boolean; maxSeconds: string; maxRamMib: string }): { params: Uint8Array } | { problem: string } {
  const seconds = positive(v.maxSeconds);
  const mib = positive(v.maxRamMib);
  if (!v.runId) return { problem: "Select a run." };
  if (seconds === null || mib === null) return { problem: "Verification budget: positive seconds and MiB are required." };
  return {
    params: toBinary(PostCaptureJobCommandSchema, message(PostCaptureJobCommandSchema, {
      runId: v.runId, chainVerify: v.mode === "off" ? "off" : "strict", offline: v.mode === "strict-cache", resumeVerification: v.resume,
      maxSeconds: seconds, maxRamBytes: mib * 1_048_576n,
    })),
  };
}

/** DEC-07: a new boot flow's offline preflight; its TTL and the run's duration are fixed here
 *  (empty TTL: the registry's value; the registry checks the bounds). */
export function bootOfflineCommand(mode: "econ4h" | "shadow" | "market", ttlSeconds: string, duration: string): { params: Uint8Array } | { problem: string } {
  const ttl = ttlSeconds.trim() === "" ? undefined : positive(ttlSeconds);
  if (ttl === null) return { problem: "TTL: whole seconds within the registry bounds, or empty for its value." };
  return { params: toBinary(BootOfflineCommandSchema, message(BootOfflineCommandSchema, { mode, ttlSeconds: ttl, duration })) };
}

const TOKEN = /^[0-9a-f]{16}$/;
/** DEC-07: an imported PROVIDER_EVIDENCE is named by its record id; the controller resolves it. */
const EVIDENCE_ID = /^[0-9a-f]{16}$/;

/** DEC-07: the operator's approval of exactly this offline token — the network audit reads Mainnet
 *  read-only with the operator's imported provider evidence (its record id). */
export function bootNetworkCommand(v: { flowId: string; token: string; genesisHash: string; providerEvidence: string; captureSource: "logs" | "transactions" }): { params: Uint8Array } | { problem: string } {
  if (!TOKEN.test(v.token)) return { problem: "No offline token to approve." };
  if (!/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(v.genesisHash.trim())) return { problem: "Genesis hash: the base58 Mainnet genesis pin." };
  if (!EVIDENCE_ID.test(v.providerEvidence.trim())) return { problem: "Provider evidence: the record id printed by p0ctl boot evidence import (16 hex digits), never a path." };
  return {
    params: toBinary(BootNetworkCommandSchema, message(BootNetworkCommandSchema, {
      flowId: v.flowId, approvedToken: v.token, genesisHash: v.genesisHash.trim(), providerEvidence: v.providerEvidence.trim(), captureSource: v.captureSource,
    })),
  };
}

/** DEC-07: the operator's approval of exactly this network token — the start of the flow's
 *  duration and capture source (online strict verification is a terminal action). */
export function bootStartCommand(v: { flowId: string; token: string; duration: string; captureSource: "logs" | "transactions"; chainVerify: "off" }): { params: Uint8Array } | { problem: string } {
  if (!TOKEN.test(v.token)) return { problem: "No READY network audit to approve." };
  return {
    params: toBinary(BootStartCommandSchema, message(BootStartCommandSchema, {
      flowId: v.flowId, approvedToken: v.token, duration: v.duration, captureSource: v.captureSource, chainVerify: v.chainVerify,
    })),
  };
}

/** S1-02: a data source's mode (`p0ctl source set`); the running owner applies or refuses it. */
export function sourceCommand(unit: string, mode: "off" | "observe" | "active"): { params: Uint8Array } | { problem: string } {
  if (!/^(?:wss|geyser:[a-z0-9-]{1,32})$/.test(unit)) return { problem: "Unknown source unit." };
  return { params: toBinary(SourceControlCommandSchema, message(SourceControlCommandSchema, { unit, mode })) };
}

/** DEC-13/DEC-04/DEC-11: the operands of `p0ctl capability set LIVE OFF|LIVE` or `… HOLD_B|JITO_BUNDLE
 * OFF|OBSERVE|ACTIVE`; the owner decides what is effective (LIVE stays BLOCKED until a separate
 * FE-06 authorization, HOLD_B ACTIVE needs the user's N and the evidence, JITO_BUNDLE ACTIVE a
 * qualified bundle simulator). */
export function capabilityCommand(capability: "LIVE", requested: "OFF" | "LIVE"): { params: Uint8Array };
export function capabilityCommand(capability: "HOLD_B" | "JITO_BUNDLE", requested: "OFF" | "OBSERVE" | "ACTIVE"): { params: Uint8Array };
export function capabilityCommand(capability: string, requested: string): { params: Uint8Array } {
  return { params: toBinary(CapabilityControlCommandSchema, message(CapabilityControlCommandSchema, { capability, requested })) };
}

async function csrf(): Promise<string> {
  const r = await fetch("/api/v1/session", { ...options, signal: AbortSignal.timeout(4000) });
  const token = r.headers.get("x-csrf-token");
  if (!r.ok || !token) throw new Error("Session unavailable");
  return token;
}
/**
 * Review L7: an open or unknown receipt never locks the panel for good — the next command waits
 * only while a command is being sent. Its state stays visible, and `recheck` looks it up again;
 * the job list (project0.analytics.jobs) is what a new command is decided on.
 */
function decide(id: string, action: JobAction, phase: CommandPhase, detail: string): void {
  useAnalyticsControl.setState({ busy: false, receipt: { id, action, phase: CommandPhase[phase] ?? "UNKNOWN", detail } });
}

/** One lookup of the shown receipt (after UNKNOWN or an open phase); never a new POST. */
export async function recheck(): Promise<void> {
  const r = useAnalyticsControl.getState().receipt;
  if (!r || !["UNKNOWN", "PENDING", "PENDING_RECONCILE"].includes(r.phase)) return;
  try { await lookup(r.id, r.action); } catch { /* the receipt stays as it is */ }
}
/** One targeted lookup, only after a lost acknowledgement; never a polling loop. */
async function lookup(id: string, action: JobAction): Promise<void> {
  const r = await fetch(`/api/v1/command?project=${ProjectId.PROJECT_0}&request_id=${encodeURIComponent(id)}`, { ...options, signal: AbortSignal.timeout(6000) });
  if (!r.ok) throw new Error(`Command lookup HTTP ${r.status}`);
  const c = fromBinary(CommandLookupResponseSchema, new Uint8Array(await r.arrayBuffer()));
  decide(id, action, c.found ? c.phase : CommandPhase.UNKNOWN, c.detail);
}

/** One request per user action; a start's request id becomes the job id. */
/** `configVersion`: the CTRL-03 CAS of a source change (the version the projection showed). */
export async function analyticsCommand(action: JobAction, args: { resourceId?: string; params?: Uint8Array; configVersion?: bigint }): Promise<void> {
  if (useAnalyticsControl.getState().busy) return;
  const id = crypto.randomUUID();
  useAnalyticsControl.setState({ busy: true, receipt: { id, action, phase: "SENDING", detail: "" } });
  let submitted = false;
  try {
    const token = await csrf();
    const body = toBinary(CommandRequestSchema, message(CommandRequestSchema, {
      project: ProjectId.PROJECT_0, requestId: id, action, resourceId: args.resourceId ?? "", params: args.params ?? new Uint8Array(),
      expected: args.configVersion == null ? undefined : { version: { case: "configVersion", value: args.configVersion } },
      issuedAtUnixNs: BigInt(Date.now()) * 1_000_000n,
    }));
    submitted = true;
    const r = await fetch("/api/v1/command", { ...options, method: "POST", headers: { "Content-Type": "application/x-protobuf", "X-CSRF-Token": token }, body, signal: AbortSignal.timeout(60000) });
    if (!r.ok) throw new Error(`Command HTTP ${r.status}`);
    const c = fromBinary(CommandResponseSchema, new Uint8Array(await r.arrayBuffer()));
    if (c.status !== CommandStatus.ACCEPTED) useAnalyticsControl.setState({ busy: false, receipt: { id, action, phase: "REJECTED", detail: c.detail } });
    else decide(id, action, c.phase, c.detail);
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    if (submitted) {
      try { await lookup(id, action); return; } catch { /* unknown outcome, never replay POST */ }
      useAnalyticsControl.setState({ busy: false, receipt: { id, action, phase: "UNKNOWN", detail } });
      return;
    }
    useAnalyticsControl.setState({ busy: false, receipt: { id, action, phase: "FAILED", detail } });
  }
}
