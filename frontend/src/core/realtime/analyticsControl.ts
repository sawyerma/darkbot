import { create as message, fromBinary, toBinary } from "@bufbuild/protobuf";
import { create } from "zustand";
import { CommandLookupResponseSchema, CommandPhase, CommandRequestSchema, CommandResponseSchema, CommandStatus } from "../generated/darkbot/control/v1/commands_pb";
import { ProjectId } from "../generated/darkbot/control/v1/topics_pb";
import { AutostartGrantCommandSchema, AutostartPreviewCommandSchema, BacktestJobCommandSchema, BootNetworkCommandSchema, BootOfflineCommandSchema, BootStartCommandSchema, H0JobCommandSchema, OptimizeJobCommandSchema, PostCaptureJobCommandSchema, ProviderCredentialCommandSchema, ProviderDescriptorCommandSchema, ProviderPriorityCommandSchema, ProviderQualifyCommandSchema, ProviderSelectCommandSchema } from "../generated/darkbot/project0/v1/analytics_pb";
import { CapabilityControlCommandSchema, SourceControlCommandSchema } from "../generated/darkbot/project0/v1/owner_pb";

/**
 * B03/U-9: the analytics job commands (POST /api/v1/command → gateway → `darkbotctl p0 job` →
 * the same `p0ctl job` a terminal runs). ACCEPTED/COMPLETED only means the controller recorded
 * the job and started its worker; the job's progress and end are its state in
 * project0.analytics.jobs. A lost acknowledgement is looked up once, never posted again.
 */
export type JobAction = "project0.analytics.backtest.start" | "project0.analytics.optimize.start" | "project0.analytics.h0.start" | "project0.analytics.job.cancel"
  | "project0.capture.verify.start" | "project0.source.set" | "project0.capability.set"
  | "project0.boot.offline" | "project0.boot.network" | "project0.boot.start" | "project0.boot.cancel"
  | "project0.autostart.preview" | "project0.autostart.grant" | "project0.autostart.revoke"
  | ProviderAction;
/** DEC-18: the provider registry's changes (`p0ctl provider …`), effective at the next controlled start. */
export type ProviderAction = "project0.provider.add" | "project0.provider.update" | "project0.provider.remove"
  | "project0.provider.enable" | "project0.provider.disable" | "project0.provider.priority" | "project0.provider.select"
  | "project0.provider.unselect" | "project0.provider.qualify" | "project0.provider.credential.set"
  | "project0.provider.credential.rotate" | "project0.provider.credential.revoke";
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

/** H0: the typed request over every saved run — the registry's window and the operator's budget
 *  (no hidden default); the minimum sample stays a terminal option (the user's decision). */
export function h0Command(p: Pick<PointInput, "maxSeconds" | "maxRamMib">): { params: Uint8Array } | { problem: string } {
  const seconds = positive(p.maxSeconds);
  const mib = positive(p.maxRamMib);
  if (seconds === null || mib === null) return { problem: "Budget: positive seconds and MiB are required (no hidden default)." };
  return { params: toBinary(H0JobCommandSchema, message(H0JobCommandSchema, { windowDays: 0n, maxSeconds: seconds, maxRamBytes: mib * 1_048_576n })) };
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

/** A08/F-02, DEC-08: the GUI's verification modes — off, or a history read from its checkpoint
 *  (an online history read is a terminal action with the operator's provider evidence). */
export type VerifyMode = "off" | "index-cache" | "strict-cache" | "crosscheck-cache";

/** A08/F-02, DEC-08: verification off, or index|strict|crosscheck from the checkpoint. */
export function verifyCommand(v: { runId: string; mode: VerifyMode; resume: boolean; maxSeconds: string; maxRamMib: string }): { params: Uint8Array } | { problem: string } {
  const seconds = positive(v.maxSeconds);
  const mib = positive(v.maxRamMib);
  if (!v.runId) return { problem: "Select a run." };
  if (seconds === null || mib === null) return { problem: "Verification budget: positive seconds and MiB are required." };
  return {
    params: toBinary(PostCaptureJobCommandSchema, message(PostCaptureJobCommandSchema, {
      runId: v.runId, chainVerify: v.mode === "off" ? "off" : v.mode.replace(/-cache$/, ""), offline: v.mode !== "off", resumeVerification: v.resume,
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
 *  duration and capture source (online strict verification is a terminal action). DEC-16: an
 *  unbounded flow (until-stopped) starts with its qualification id. */
export function bootStartCommand(v: { flowId: string; token: string; duration: string; captureSource: "logs" | "transactions"; chainVerify: "off"; qualification?: string }): { params: Uint8Array } | { problem: string } {
  if (!TOKEN.test(v.token)) return { problem: "No READY network audit to approve." };
  const unbounded = v.duration.startsWith("until-stopped:");
  const qualification = v.qualification?.trim();
  if (unbounded && !TOKEN.test(qualification ?? "")) return { problem: "Until stopped: the qualification id (16 hex digits) is required." };
  return {
    params: toBinary(BootStartCommandSchema, message(BootStartCommandSchema, {
      flowId: v.flowId, approvedToken: v.token, duration: v.duration, captureSource: v.captureSource, chainVerify: v.chainVerify,
      qualification: unbounded ? qualification : undefined,
    })),
  };
}

const DURATION = /^[1-9]\d{0,5}[smhd]$/;
/** DEC-16: a boot flow's unbounded horizon `until-stopped:<window>:<runway>`. */
export function untilStoppedDuration(window: string, runway: string): string | null {
  return DURATION.test(window.trim()) && DURATION.test(runway.trim()) ? `until-stopped:${window.trim()}:${runway.trim()}` : null;
}

/** DEC-16: the terms of a standing autostart approval; the controller binds them to the run and
 *  records the preview with its digest (shown before any grant). */
export function autostartPreviewCommand(v: { qualification: string; window: string; runway: string; genesisHash: string; maxRuntime: string; validFor: string; captureSource: "logs" | "transactions" }): { params: Uint8Array } | { problem: string } {
  if (!TOKEN.test(v.qualification.trim())) return { problem: "Qualification: its id (16 hex digits)." };
  if (untilStoppedDuration(v.window, v.runway) === null) return { problem: "Window and runway: durations (s/m/h/d)." };
  if (!/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(v.genesisHash.trim())) return { problem: "Genesis hash: the base58 Mainnet genesis pin." };
  if (!DURATION.test(v.maxRuntime.trim())) return { problem: "Maximum runtime: a duration (s/m/h/d)." };
  if (!DURATION.test(v.validFor.trim())) return { problem: "Valid for: a duration (s/m/h/d)." };
  return {
    params: toBinary(AutostartPreviewCommandSchema, message(AutostartPreviewCommandSchema, {
      qualification: v.qualification.trim(), checkpoint: v.window.trim(), minimumRunway: v.runway.trim(), genesisHash: v.genesisHash.trim(),
      maxRuntime: v.maxRuntime.trim(), validFor: v.validFor.trim(), captureSource: v.captureSource,
    })),
  };
}

/** DEC-16: the operator confirms exactly the previewed binding by its digest. */
export function autostartGrantCommand(digest: string): { params: Uint8Array } | { problem: string } {
  if (!TOKEN.test(digest)) return { problem: "No previewed binding to confirm." };
  return { params: toBinary(AutostartGrantCommandSchema, message(AutostartGrantCommandSchema, { confirmDigest: digest })) };
}

const PROVIDER_ID = /^(?![0-9a-fA-F]{16,}$)[a-z0-9_-]{1,32}$/;
const PROVIDER_KINDS = ["rpc", "wss", "geyser", "jupiter", "jito"] as const;
/** The provider form as the operator entered it (empty: not set; an update changes only what it sets). */
export interface ProviderInput {
  id: string; kind: string; endpoint: string; auth: string; credential: string;
  priority: string; capacityRps: string; plan: string; entitlement: string; disabled: boolean;
}

/** DEC-18: an add or an update of a provider. A key never belongs in an endpoint (its credential
 *  is set separately): an endpoint with user info, a query or a fragment is refused here as by the
 *  gateway and the native registry, which decide every rule. */
export function providerDescriptorCommand(v: ProviderInput, add: boolean): { params: Uint8Array } | { problem: string } {
  const t = (s: string) => s.trim();
  if (!PROVIDER_ID.test(t(v.id))) return { problem: "Id: 1–32 of a-z, 0-9, _ or - (never a key)." };
  if (add && (!t(v.kind) || !t(v.endpoint) || !t(v.plan) || !t(v.entitlement))) return { problem: "A new provider needs its kind, endpoint, plan and entitlement." };
  if (t(v.kind) && !(PROVIDER_KINDS as readonly string[]).includes(t(v.kind))) return { problem: "Kind: rpc, wss, geyser, jupiter or jito." };
  if (t(v.endpoint) && (!/^(?:https?|wss?):\/\/[^/@?#\s]+(?:\/[^?#\s]*)?$/.test(t(v.endpoint)))) return { problem: "Endpoint: a URL without user info, query or fragment — a key is the credential's, never the endpoint's." };
  if (t(v.auth) && !/^(?:none|path|(?:query|header):[A-Za-z0-9_-]{1,64})$/.test(t(v.auth))) return { problem: "Authentication: none, path, query:NAME or header:NAME (a name, never a key)." };
  if (t(v.credential) && !PROVIDER_ID.test(t(v.credential))) return { problem: "Credential: its id (never the key itself)." };
  if (t(v.plan) && !["free", "unknown"].includes(t(v.plan))) return { problem: "Plan: free or unknown (FREE_ONLY)." };
  if (t(v.priority) && !/^\d{1,9}$/.test(t(v.priority))) return { problem: "Priority: a whole number." };
  if (t(v.capacityRps) && !/^[1-9]\d{0,8}$/.test(t(v.capacityRps))) return { problem: "Capacity: requests per second, at least 1." };
  if (!add && v.disabled) return { problem: "Disabling is its own request." };
  const opt = (s: string) => (t(s) ? t(s) : undefined);
  return {
    params: toBinary(ProviderDescriptorCommandSchema, message(ProviderDescriptorCommandSchema, {
      id: t(v.id), kind: opt(v.kind), endpoint: opt(v.endpoint), auth: opt(v.auth), credential: opt(v.credential),
      priority: t(v.priority) ? Number(t(v.priority)) : undefined, capacityRps: t(v.capacityRps) ? BigInt(t(v.capacityRps)) : undefined,
      plan: opt(v.plan), entitlement: opt(v.entitlement), disabled: add && v.disabled,
    })),
  };
}

/** DEC-18: a provider's priority among its kind. */
export function providerPriorityCommand(id: string, priority: string): { params: Uint8Array } | { problem: string } {
  if (!PROVIDER_ID.test(id) || !/^\d{1,9}$/.test(priority.trim())) return { problem: "Priority: a whole number." };
  return { params: toBinary(ProviderPriorityCommandSchema, message(ProviderPriorityCommandSchema, { id, priority: Number(priority.trim()) })) };
}

/** DEC-18: the selected provider of a kind (the Geyser source unit names its own). */
export function providerSelectCommand(kind: string, id: string): { params: Uint8Array } | { problem: string } {
  if (!["rpc", "wss", "jupiter", "jito"].includes(kind) || !PROVIDER_ID.test(id)) return { problem: "Select a provider of rpc, wss, jupiter or jito." };
  return { params: toBinary(ProviderSelectCommandSchema, message(ProviderSelectCommandSchema, { kind, id })) };
}

/** DEC-18: a provider's qualification by a stored provider evidence record (or "-" to clear it). */
export function providerQualifyCommand(id: string, evidence: string): { params: Uint8Array } | { problem: string } {
  if (!PROVIDER_ID.test(id) || !(evidence === "-" || /^[0-9a-f]{16}$/.test(evidence.trim()))) return { problem: "Evidence: a record id (16 hex digits) or - to clear." };
  return { params: toBinary(ProviderQualifyCommandSchema, message(ProviderQualifyCommandSchema, { id, evidence: evidence.trim() })) };
}

/** DEC-18 (P0-019): a credential's secret, set or rotated — carried in the request's parameters
 *  only (the gateway separates it before any record and hands it to the native controller's
 *  standard input); never kept in a store, the receipt or the browser's storage. */
export function providerCredentialCommand(credential: string, secret: string): { params: Uint8Array } | { problem: string } {
  if (!PROVIDER_ID.test(credential)) return { problem: "Credential: its id." };
  if (secret === "" || /[\u0000-\u001f\u007f]/.test(secret)) return { problem: "Key: the provider's key, one line." };
  return { params: toBinary(ProviderCredentialCommandSchema, message(ProviderCredentialCommandSchema, { credential, secret: new TextEncoder().encode(secret) })) };
}

/** The capabilities of mode OFF|OBSERVE|ACTIVE (`capabilityCommand`). */
export type ModeCapability = "HOLD_B" | "JITO_BUNDLE" | "THRESHOLD_TRIGGER" | "INVENTORY_REBALANCING" | "LOCAL_FEE_MODEL";

/** S1-02: a data source's mode (`p0ctl source set`); the running owner applies or refuses it. */
export function sourceCommand(unit: string, mode: "off" | "observe" | "active"): { params: Uint8Array } | { problem: string } {
  if (!/^(?:wss|geyser:[a-z0-9-]{1,32})$/.test(unit)) return { problem: "Unknown source unit." };
  return { params: toBinary(SourceControlCommandSchema, message(SourceControlCommandSchema, { unit, mode })) };
}

/** DEC-13/DEC-04/DEC-11/DEC-06/B01/P6-MT.10: the operands of `p0ctl capability set LIVE OFF|LIVE`
 * or `… HOLD_B|JITO_BUNDLE|THRESHOLD_TRIGGER|INVENTORY_REBALANCING|LOCAL_FEE_MODEL
 * OFF|OBSERVE|ACTIVE`; the owner decides what is effective (LIVE stays BLOCKED until a separate
 * FE-06 authorization, HOLD_B ACTIVE needs the user's N and the evidence, JITO_BUNDLE ACTIVE a
 * qualified bundle simulator, THRESHOLD_TRIGGER ACTIVE comes only from OBSERVE with N comparable
 * ticks without a miss, INVENTORY_REBALANCING ACTIVE is never effective — a rebalance trade has
 * no authorization of its own —, LOCAL_FEE_MODEL ACTIVE only in a class the sealed parity
 * qualifies). */
/** P8-CTRL.6 (review M2): stated with the owner epoch this view showed (0: unknown) — the gateway
 * takes every request but a move to OFF only with it, so a view from before an owner restart
 * never re-arms LIVE or a mode. */
export function capabilityCommand(capability: "LIVE", requested: "OFF" | "LIVE", ownerEpoch: bigint | null): { params: Uint8Array };
export function capabilityCommand(capability: ModeCapability, requested: "OFF" | "OBSERVE" | "ACTIVE", ownerEpoch: bigint | null): { params: Uint8Array };
export function capabilityCommand(capability: string, requested: string, ownerEpoch: bigint | null): { params: Uint8Array } {
  return { params: toBinary(CapabilityControlCommandSchema, message(CapabilityControlCommandSchema, { capability, requested, ownerEpoch: ownerEpoch ?? 0n })) };
}

/** P8-CTRL.1: the economic switches, in the order the owner names a missing one. */
export const ECON_SWITCHES = ["ECON_LIVE", "ECON_ARM", "ECON_SIGN", "ECON_SEND", "ECON_ATTEMPT"] as const;
/** P8-CTRL.1: one switch per configured signer, `WALLET:<base58 public key>`. */
export const WALLET_PREFIX = "WALLET:";

/** P8-CTRL.1/.3/.8: the operands of `p0ctl capability set ECON_*|WALLET:<key> ENABLED|DISABLED`
 * or of `p0ctl live grant|revoke --digest D` — stated with the owner epoch this view showed (the
 * gateway refuses an economic request without one, the owner one of another epoch). ENABLED is a
 * partial permission, never a command to sign or send; a grant is refused by the owner while the
 * operator authentication is undecided. */
export function economicCommand(capability: string, requested: string, ownerEpoch: bigint | null): { params: Uint8Array } | { problem: string } {
  if (ownerEpoch == null || ownerEpoch <= 0n) return { problem: "Owner epoch unknown." };
  const onOff = requested === "ENABLED" || requested === "DISABLED";
  const valid = ((ECON_SWITCHES as readonly string[]).includes(capability) && onOff)
    || (capability.startsWith(WALLET_PREFIX) && /^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(capability.slice(WALLET_PREFIX.length)) && onOff)
    || ((capability === "FE06_GRANT" || capability === "FE06_REVOKE") && /^[0-9a-f]{64}$/.test(requested));
  if (!valid) return { problem: "Not an economic switch, a signer or an FE-06 digest." };
  return { params: toBinary(CapabilityControlCommandSchema, message(CapabilityControlCommandSchema, { capability, requested, ownerEpoch })) };
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
